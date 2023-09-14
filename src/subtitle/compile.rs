pub fn trivial<'a>(sline: &'a super::Sline, counter: &mut i32) -> super::ass::Event<'a> {
    let event = super::ass::Event {
        start: sline.start,
        duration: sline.duration,
        layer_index: sline.layer_index,
        style_index: sline.style_index,
        margins: sline.margins,
        text: sline.text.as_str(),
        read_order: *counter,
        name: "",
        effect: "",
    };

    *counter += 1;
    event
}
