use harumi::{Document, FieldType, TextFieldOptions};

#[test]
fn create_text_field_then_read_back() {
    let mut doc = Document::new((200.0, 200.0)).unwrap();
    doc.add_text_field(
        1,
        "username",
        [10.0, 100.0, 100.0, 20.0],
        &TextFieldOptions {
            default_value: "John Doe".to_string(),
            multiline: false,
            read_only: false,
        },
    )
    .unwrap();

    let pdf_bytes = doc.save_to_bytes().unwrap();
    let reloaded = Document::from_bytes(&pdf_bytes).unwrap();

    let fields = reloaded.form_fields().unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].name, "username");
    assert_eq!(fields[0].field_type, FieldType::Text);
    assert_eq!(fields[0].value, "John Doe");
}

#[test]
fn create_checkbox_checked() {
    let mut doc = Document::new((200.0, 200.0)).unwrap();
    doc.add_checkbox(1, "agree", [10.0, 100.0, 20.0, 20.0], true)
        .unwrap();

    let pdf_bytes = doc.save_to_bytes().unwrap();
    let reloaded = Document::from_bytes(&pdf_bytes).unwrap();

    let fields = reloaded.form_fields().unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].name, "agree");
    assert_eq!(fields[0].field_type, FieldType::Checkbox);
    assert_eq!(fields[0].value, "Yes");
}

#[test]
fn create_checkbox_unchecked() {
    let mut doc = Document::new((200.0, 200.0)).unwrap();
    doc.add_checkbox(1, "agree", [10.0, 100.0, 20.0, 20.0], false)
        .unwrap();

    let pdf_bytes = doc.save_to_bytes().unwrap();
    let reloaded = Document::from_bytes(&pdf_bytes).unwrap();

    let fields = reloaded.form_fields().unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].field_type, FieldType::Checkbox);
    assert_eq!(fields[0].value, "Off");
}

#[test]
fn create_radio_group() {
    let mut doc = Document::new((200.0, 200.0)).unwrap();
    doc.add_radio_group(
        1,
        "color",
        &[
            ("red", [10.0, 150.0, 20.0, 20.0]),
            ("green", [40.0, 150.0, 20.0, 20.0]),
            ("blue", [70.0, 150.0, 20.0, 20.0]),
        ],
        Some("green"),
    )
    .unwrap();

    let pdf_bytes = doc.save_to_bytes().unwrap();
    let reloaded = Document::from_bytes(&pdf_bytes).unwrap();

    let fields = reloaded.form_fields().unwrap();
    // Radio groups: collect_fields_recursive traverses /Kids and returns only leaf fields
    // So we see 3 child fields (red, green, blue) with parent name "color"
    assert_eq!(fields.len(), 3);
    // All three children should have the parent name prepended
    let color_fields: Vec<_> = fields
        .iter()
        .filter(|f| f.name.starts_with("color"))
        .collect();
    assert_eq!(color_fields.len(), 3);
    // Find the selected one
    let selected = color_fields.iter().find(|f| f.value == "green");
    assert!(selected.is_some());
}

#[test]
fn fill_created_field() {
    let mut doc = Document::new((200.0, 200.0)).unwrap();
    doc.add_text_field(
        1,
        "username",
        [10.0, 100.0, 100.0, 20.0],
        &TextFieldOptions::default(),
    )
    .unwrap();

    let pdf_bytes = doc.save_to_bytes().unwrap();
    let mut reloaded = Document::from_bytes(&pdf_bytes).unwrap();

    // Fill the field
    let count = reloaded.fill_form(&[("username", "Alice Smith")]).unwrap();
    assert_eq!(count, 1);

    let pdf_bytes2 = reloaded.save_to_bytes().unwrap();
    let reloaded2 = Document::from_bytes(&pdf_bytes2).unwrap();

    let fields = reloaded2.form_fields().unwrap();
    assert_eq!(fields[0].value, "Alice Smith");
}

#[test]
fn create_multiline_text_field() {
    let mut doc = Document::new((200.0, 200.0)).unwrap();
    doc.add_text_field(
        1,
        "comments",
        [10.0, 100.0, 150.0, 50.0],
        &TextFieldOptions {
            default_value: String::new(),
            multiline: true,
            read_only: false,
        },
    )
    .unwrap();

    let pdf_bytes = doc.save_to_bytes().unwrap();
    let reloaded = Document::from_bytes(&pdf_bytes).unwrap();

    let fields = reloaded.form_fields().unwrap();
    assert_eq!(fields[0].name, "comments");
    assert_eq!(fields[0].field_type, FieldType::Text);
}

#[test]
fn radio_group_default_selection() {
    let mut doc = Document::new((200.0, 200.0)).unwrap();
    // Don't specify selected, should default to first option
    doc.add_radio_group(
        1,
        "size",
        &[
            ("small", [10.0, 150.0, 20.0, 20.0]),
            ("medium", [40.0, 150.0, 20.0, 20.0]),
            ("large", [70.0, 150.0, 20.0, 20.0]),
        ],
        None,
    )
    .unwrap();

    let pdf_bytes = doc.save_to_bytes().unwrap();
    let reloaded = Document::from_bytes(&pdf_bytes).unwrap();

    let fields = reloaded.form_fields().unwrap();
    // Find the selected child field (should be "small" selected by default)
    let size_fields: Vec<_> = fields
        .iter()
        .filter(|f| f.name.starts_with("size"))
        .collect();
    let selected = size_fields.iter().find(|f| f.value == "small");
    assert!(
        selected.is_some(),
        "first option 'small' should be selected by default"
    );
}
