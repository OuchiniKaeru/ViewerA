use std::io::{Cursor, Write};

use viewera_lib::files::{read_workbook, read_worksheet_range};
use viewera_lib::models::RootEntry;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn write_workbook(directory: &std::path::Path) -> String {
    let path = directory.join("report.xlsx");
    let parts = [
        (
            "[Content_Types].xml",
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
        ),
        (
            "xl/workbook.xml",
            r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
        ),
        (
            "xl/worksheets/sheet1.xml",
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" s="1" t="inlineStr"><is><t>Name</t></is></c><c r="B1" t="inlineStr"><is><t>Value</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>Item</t></is></c><c r="B2"><v>42</v></c></row></sheetData></worksheet>"#,
        ),
        (
            "xl/styles.xml",
            r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><color rgb="FFFF0000"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="solid"><fgColor rgb="FFFFFF00"/><bgColor indexed="64"/></patternFill></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="0" fontId="1" fillId="1" borderId="0" xfId="0" applyFont="1" applyFill="1"/></cellXfs></styleSheet>"#,
        ),
    ];
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, contents) in parts {
        archive
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        archive.write_all(contents.as_bytes()).unwrap();
    }
    std::fs::write(&path, archive.finish().unwrap().into_inner()).unwrap();
    path.to_string_lossy().into_owned()
}

fn root(path: &std::path::Path) -> RootEntry {
    RootEntry {
        id: "workbook-test".to_owned(),
        label: "Test".to_owned(),
        path: path.to_string_lossy().into_owned(),
        added_at: String::new(),
    }
}

#[test]
fn reads_workbook_sheets_and_a_requested_cell_range() {
    let directory = tempfile::tempdir().unwrap();
    write_workbook(directory.path());
    let root = root(directory.path());

    let workbook = read_workbook(&root, "report.xlsx").unwrap();
    let range = read_worksheet_range(&root, "report.xlsx", "Data", 0, 0, 2, 2).unwrap();

    assert_eq!(workbook.sheets.len(), 1);
    assert_eq!(workbook.sheets[0].name, "Data");
    assert_eq!(range.rows[0], ["Name", "Value"]);
    assert_eq!(range.rows[1], ["Item", "42"]);
    assert_eq!(range.styles.len(), 1);
    assert_eq!(range.styles[0].row, 0);
    assert_eq!(range.styles[0].column, 0);
    assert!(range.styles[0].bold);
    assert_eq!(range.styles[0].font_color.as_deref(), Some("#FF0000"));
    assert_eq!(range.styles[0].fill_color.as_deref(), Some("#FFFF00"));
}

#[test]
fn caps_requested_worksheet_ranges() {
    let directory = tempfile::tempdir().unwrap();
    write_workbook(directory.path());
    let root = root(directory.path());

    assert!(read_worksheet_range(&root, "report.xlsx", "Data", 0, 0, 10_000, 10_000).is_err());
}
