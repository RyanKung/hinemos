use yew::prelude::*;

#[function_component(WorldSketch)]
fn world_sketch() -> Html {
    html! {
        <>
            <pre>{"============================================================\n                  ISLAND HARBOR CROSSING\n============================================================\n\n          [Room] ------- <Me>\n            |              |\n      harbor lane       [Guild]\n                           |\n                     {bulletin board}"}</pre>
            <div class="card-footer">
                <span>{"observe"}</span>
                <span>{"move"}</span>
                <span>{"trade"}</span>
            </div>
        </>
    }
}

fn main() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(root) = document.get_element_by_id("world-sketch") else {
        return;
    };
    yew::Renderer::<WorldSketch>::with_root(root).render();
}
