-- One-shot deployment migration for PR #30.
--
-- Purpose:
--   Enforce one payment request per parcel operator command before deploying
--   code that treats /parcel request-payment as idempotent by command id.
--
-- Safety policy:
--   - If one operator command has multiple paid payment requests, abort.
--     Those rows represent real ledger transfers and need manual refund/audit.
--   - Otherwise keep one canonical row per operator_command_id.
--   - Prefer paid over pending, pending over cancelled, then lowest id.
--   - Archive inbox items for deleted duplicate payment requests.

begin;

lock table payment_requests in exclusive mode;
lock table inbox_items in share row exclusive mode;
lock table operator_commands in share row exclusive mode;

do $$
begin
    if exists (
        select 1
        from payment_requests
        where status = 'paid'
        group by operator_command_id
        having count(*) > 1
    ) then
        raise exception
            'cannot add payment_requests_operator_command_unique_idx: multiple paid payment_requests exist for one operator_command_id';
    end if;
end $$;

create temp table payment_request_duplicates on commit drop as
with ranked as (
    select id,
           operator_command_id,
           first_value(id) over (
               partition by operator_command_id
               order by case status
                            when 'paid' then 0
                            when 'pending' then 1
                            else 2
                        end,
                        id
           ) as canonical_id
    from payment_requests
)
select id, canonical_id
from ranked
where id <> canonical_id;

update inbox_items item
set status = 'archived',
    source_kind = 'payment_request_duplicate',
    payload = item.payload || jsonb_build_object(
        'duplicateOfPaymentRequestId', duplicates.canonical_id
    )
from payment_request_duplicates duplicates
where item.source_kind = 'payment_request'
  and item.source_id = duplicates.id;

delete from payment_requests request
using payment_request_duplicates duplicates
where request.id = duplicates.id;

update operator_commands command
set status = 'handled'
where exists (
    select 1
    from payment_requests request
    where request.operator_command_id = command.id
);

create unique index if not exists payment_requests_operator_command_unique_idx
on payment_requests (operator_command_id);

commit;
