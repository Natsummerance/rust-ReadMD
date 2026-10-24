//! Stable machine-readable codes carried next to the legacy Chinese
//! `error` / `note` / `warns` text, so the frontend can localise every
//! failure, notice and export warning (`error.<code>`, `note.<code>`,
//! `warn.<code>` in `assets/i18n/*.json`).
//!
//! Only additive: existing keys stay byte-identical, the codes ride along as
//! `error_code`, `note_code` and `warn_items`.

use serde_json::{json, Value};

// ---- error_code ------------------------------------------------------------
pub const FILE_NOT_FOUND: &str = "file_not_found";
pub const CONVERSION_FAILED: &str = "conversion_failed";
pub const UNSUPPORTED_FORMAT: &str = "unsupported_format";
pub const LEGACY_OFFICE_PARSE_FAILED: &str = "legacy_office_parse_failed";
pub const OCR_FAILED: &str = "ocr_failed";
pub const OCR_BUSY: &str = "ocr_busy";
pub const OCR_TIMEOUT: &str = "ocr_timeout";
pub const OCR_NO_ENGINE: &str = "ocr_no_engine";
pub const ENCODING_UNREPRESENTABLE: &str = "encoding_unrepresentable";
pub const ENCODING_UNKNOWN: &str = "encoding_unknown";
pub const DIALOG_UNAVAILABLE: &str = "dialog_unavailable";
pub const PATH_NOT_FOUND: &str = "path_not_found";
pub const OPEN_FAILED: &str = "open_failed";
pub const PRESET_NAME_CONFLICT: &str = "preset_name_conflict";
pub const INTERNAL_ERROR: &str = "internal_error";
pub const ZIP_CORRUPT: &str = "zip_corrupt";
pub const ZIP_TOO_LARGE: &str = "zip_too_large";
pub const ZIP_UNSUPPORTED: &str = "zip_unsupported";
pub const CANCELLED: &str = "cancelled";
pub const OPENAPI_INVALID: &str = "openapi_invalid";
pub const OPENAPI_BUDGET: &str = "openapi_budget";
pub const OPENAPI_BUSY: &str = "openapi_busy";
pub const OPENAPI_VERSION: &str = "openapi_version";
pub const OPENAPI_REGION_INVALID: &str = "openapi_region_invalid";
pub const OPENAPI_LOCAL_CHANGES: &str = "openapi_local_changes";
pub const LATEX_ENGINE_UNAVAILABLE: &str = "latex_engine_unavailable";
pub const LATEX_INVALID_ENGINE: &str = "latex_invalid_engine";
pub const LATEX_INVALID_OPTIONS: &str = "latex_invalid_options";
pub const LATEX_WORKSPACE_FAILED: &str = "latex_workspace_failed";
pub const LATEX_TIMEOUT: &str = "latex_timeout";
pub const LATEX_OUTPUT_LIMIT: &str = "latex_output_limit";
pub const LATEX_PROCESS_FAILED: &str = "latex_process_failed";
pub const LATEX_INVALID_OUTPUT: &str = "latex_invalid_output";
pub const LATEX_COMPILATION_FAILED: &str = "latex_compilation_failed";
pub const LATEX_REFERENCES_UNSETTLED: &str = "latex_references_unsettled";
pub const LATEX_SAVE_FAILED: &str = "latex_save_failed";
pub const LATEX_BUSY: &str = "latex_busy";
pub const TYPST_ENGINE_UNAVAILABLE: &str = "typst_engine_unavailable";
pub const TYPST_WORKSPACE_FAILED: &str = "typst_workspace_failed";
pub const TYPST_TIMEOUT: &str = "typst_timeout";
pub const TYPST_OUTPUT_LIMIT: &str = "typst_output_limit";
pub const TYPST_PROCESS_FAILED: &str = "typst_process_failed";
pub const TYPST_INVALID_OUTPUT: &str = "typst_invalid_output";
pub const TYPST_COMPILATION_FAILED: &str = "typst_compilation_failed";
pub const TYPST_SAVE_FAILED: &str = "typst_save_failed";
pub const TYPST_BUSY: &str = "typst_busy";
pub const TYPST_FORMULA_FAILED: &str = "typst_formula_failed";
pub const TYPST_INVALID_CONTENT: &str = "typst_invalid_content";
pub const DOCX_NESTED_FOOTNOTE: &str = "docx_nested_footnote";
pub const DOCX_NOTES_INVALID: &str = "docx_notes_invalid";
pub const DOCX_CITATION_INVALID: &str = "docx_citation_invalid";
pub const DOCX_CITATION_CONFLICT: &str = "docx_citation_conflict";
pub const DOCX_CITATION_RESOURCES_INVALID: &str = "docx_citation_resources_invalid";
pub const PUBLICATION_NUMBERING_INVALID: &str = "publication_numbering_invalid";
pub const PUBLICATION_NUMBERING_BUDGET: &str = "publication_numbering_budget";
pub const PUBLICATION_NUMBERING_HIERARCHY: &str = "publication_numbering_hierarchy";
pub const PUBLICATION_NUMBERING_CHAPTER_MISSING: &str = "publication_numbering_chapter_missing";
pub const DOCX_NUMBERING_INVALID: &str = "docx_numbering_invalid";
pub const DOCX_NUMBERING_BUDGET: &str = "docx_numbering_budget";
pub const DOCX_NUMBERING_UNREPRESENTABLE: &str = "docx_numbering_unrepresentable";
pub const DOCX_NUMBERING_IMPORT_CONFLICT: &str = "docx_numbering_import_conflict";
pub const PUBLICATION_NOTE_POLICY_INVALID: &str = "publication_note_policy_invalid";
pub const PUBLICATION_NOTE_POLICY_UNSUPPORTED: &str = "publication_note_policy_unsupported";
pub const PUBLICATION_NOTE_BINDING_MISMATCH: &str = "publication_note_binding_mismatch";
pub const DOCX_NOTE_SETTINGS_INVALID: &str = "docx_note_settings_invalid";
pub const DOCX_NOTE_SETTINGS_UNSUPPORTED: &str = "docx_note_settings_unsupported";
pub const AUTHOR_INLINE_TEXT_INVALID: &str = "author_inline_text_invalid";
pub const AUTHOR_INLINE_TEXT_UNSUPPORTED: &str = "author_inline_text_unsupported";
pub const AUTHOR_INLINE_TEXT_TARGET: &str = "author_inline_text_target";
pub const AUTHOR_INLINE_TEXT_BUDGET: &str = "author_inline_text_budget";
pub const AUTHOR_INLINE_TEXT_PDF_SHAPING_UNSUPPORTED: &str = "author_inline_text_pdf_shaping_unsupported";
pub const PUBLICATION_LAYOUT_GEOMETRY: &str = "publication_layout_geometry";
pub const PUBLICATION_NOTE_LAYOUT: &str = "publication_note_layout";
pub const PUBLICATION_FONTS_UNAVAILABLE: &str = "publication_fonts_unavailable";
pub const PUBLICATION_RUNNING_OVERLAP: &str = "publication_running_overlap";
pub const PUBLICATION_LOCAL_INVALID: &str = "publication_local_style_invalid";
pub const PUBLICATION_LINE_SPACING_UNSUPPORTED: &str = "publication_line_spacing_unsupported";
pub const PUBLICATION_TRACKING_UNSUPPORTED: &str = "publication_tracking_unsupported";
pub const PUBLICATION_BASELINE_UNSUPPORTED: &str = "publication_baseline_unsupported";
pub const EPUB_INVALID_OPTIONS: &str = "epub_invalid_options";
pub const MATH_METADATA_STALE: &str = "math_metadata_stale";
pub const MATH_METADATA_INVALID: &str = "math_metadata_invalid";
pub const MATH_DOCUMENT_DEFAULTS_CONFLICT: &str = "math_document_defaults_conflict";
pub const PUBLICATION_NAVIGATION_INVALID: &str = "publication_navigation_invalid";
pub const PUBLICATION_NAVIGATION_REFERENCE: &str = "publication_navigation_reference";
pub const PUBLICATION_NAVIGATION_BUDGET: &str = "publication_navigation_budget";
pub const PUBLICATION_LOCAL_VERSION: &str = "publication_local_style_version";
pub const PUBLICATION_LOCAL_ORPHAN: &str = "publication_local_style_orphan";
pub const PUBLICATION_LOCAL_DUPLICATE: &str = "publication_local_style_duplicate";
pub const PUBLICATION_LOCAL_TARGET: &str = "publication_local_style_target";
pub const WARN_PUBLICATION_LOCAL_UNSUPPORTED: &str = "publication_local_style_unsupported";
pub const WARN_PUBLICATION_LAYOUT_PAGED: &str = "publication_layout_paged_unsupported";
pub const WARN_PUBLICATION_LAYOUT_PARAGRAPH: &str = "publication_layout_paragraph_unsupported";
pub const WARN_PUBLICATION_LAYOUT_NOTES: &str = "publication_layout_notes_unsupported";
pub const WARN_PUBLICATION_NUMBERING_TAB: &str = "publication_numbering_tab_unsupported";
pub const WARN_PUBLICATION_NOTE_PAGE_RESTART: &str = "publication_note_page_restart_unsupported";
pub const WARN_PUBLICATION_NOTE_SECTION_RESTART: &str = "publication_note_section_restart_unsupported";
pub const WARN_PUBLICATION_NOTE_CONTINUATION: &str = "publication_note_continuation_unsupported";
pub const WARN_DOCX_CITATION_INVALID: &str = "docx_citation_invalid";
pub const WARN_DOCX_CITATION_CONFLICT: &str = "docx_citation_conflict";
pub const WARN_DOCX_CITATION_RESOURCES_INVALID: &str = "docx_citation_resources_invalid";
pub const WARN_PUBLICATION_LAYOUT_HEADING: &str = "publication_layout_heading_unsupported";
pub const WARN_PUBLICATION_CONTINUOUS_COLUMNS_PARTIAL: &str = "publication_continuous_columns_partial";
pub const WARN_PUBLICATION_FONT_SUBSTITUTED: &str = "publication_font_substituted";

pub const PDF_BUSY: &str = "pdf_busy";
pub const PDF_CONFLICT: &str = "pdf_conflict";
pub const PDF_ENGINE_UNAVAILABLE: &str = "pdf_engine_unavailable";
pub const PDF_INVALID_OPERATION: &str = "pdf_invalid_operation";
pub const PDF_INVALID_PAGE: &str = "pdf_invalid_page";
pub const PDF_LAST_PAGE: &str = "pdf_last_page";
pub const PDF_LIMIT: &str = "pdf_limit";
pub const PDF_MISSING_GLYPH: &str = "pdf_missing_glyph";
pub const PDF_OPEN_FAILED: &str = "pdf_open_failed";
pub const PDF_OPERATION_FAILED: &str = "pdf_operation_failed";
pub const PDF_READ_ONLY: &str = "pdf_read_only";
pub const PDF_SAVE_FAILED: &str = "pdf_save_failed";
pub const PDF_SESSION_EXPIRED: &str = "pdf_session_expired";
pub const PDF_SIGNED: &str = "pdf_signed";
pub const PDF_TEXT_OVERFLOW: &str = "pdf_text_overflow";
pub const PDF_TIMEOUT: &str = "pdf_timeout";
pub const PDF_UNSUPPORTED_OBJECT: &str = "pdf_unsupported_object";
pub const PDF_WORKER_FAILED: &str = "pdf_worker_failed";
pub const CAPABILITY_PACK_INVALID: &str = "capability_pack_invalid";
pub const PUBLICATION_PDF_RICH_FORMAT_INVALID: &str = "publication_pdf_rich_format_invalid";
pub const ACADEMIC_AFFIX_UNSUPPORTED: &str = "academic_affix_unsupported";
pub const WARN_ACADEMIC_UNPRINTABLE_REFERENCE: &str = "academic_unprintable_reference";
pub const WARN_ACADEMIC_DIAGNOSTIC: &str = "academic_diagnostic";
pub const WARN_PUBLICATION_METADATA_DATE_CUSTOM_ONLY: &str = "publication_metadata_date_custom_only";
pub const WARN_PUBLICATION_METADATA_SERIES_POSITION_CUSTOM_ONLY: &str = "publication_metadata_series_position_custom_only";

pub const PUBLICATION_METADATA_ACTION: &str = "publication_metadata_action";

pub const PUBLICATION_METADATA_ALIAS_CONFLICT: &str = "publication_metadata_alias_conflict";

pub const PUBLICATION_METADATA_BUDGET: &str = "publication_metadata_budget";

pub const PUBLICATION_METADATA_CLEAR_CONFLICT: &str = "publication_metadata_clear_conflict";

pub const PUBLICATION_METADATA_DUPLICATE: &str = "publication_metadata_duplicate";

pub const PUBLICATION_METADATA_DUPLICATE_ID: &str = "publication_metadata_duplicate_id";

pub const PUBLICATION_METADATA_FIELD: &str = "publication_metadata_field";

pub const PUBLICATION_METADATA_ID: &str = "publication_metadata_id";

pub const PUBLICATION_METADATA_IDENTIFIER_ROLE: &str = "publication_metadata_identifier_role";

pub const PUBLICATION_METADATA_ISBN: &str = "publication_metadata_isbn";

pub const PUBLICATION_METADATA_LANGUAGE: &str = "publication_metadata_language";

pub const PUBLICATION_METADATA_ORCID: &str = "publication_metadata_orcid";

pub const PUBLICATION_METADATA_PATCH_SYNTAX: &str = "publication_metadata_patch_syntax";

pub const PUBLICATION_METADATA_PRIMARY: &str = "publication_metadata_primary";

pub const PUBLICATION_METADATA_REFERENCE: &str = "publication_metadata_reference";

pub const PUBLICATION_METADATA_REQUIRED: &str = "publication_metadata_required";

pub const PUBLICATION_METADATA_TYPE: &str = "publication_metadata_type";

pub const PUBLICATION_METADATA_YAML: &str = "publication_metadata_yaml";

pub const PUBLICATION_METADATA_YAML_FEATURE: &str = "publication_metadata_yaml_feature";

pub const PUBLICATION_METADATA_PDF: &str = "publication_metadata_pdf";

pub const WARN_PUBLICATION_METADATA_TITLE_FALLBACK: &str = "publication_metadata_title_fallback";

pub const WARN_PUBLICATION_METADATA_LABEL_FALLBACK: &str = "publication_metadata_label_fallback";

pub const ACADEMIC_CONTEXT: &str = "academic_context";

pub const ACADEMIC_CONTEXT_BUDGET: &str = "academic_context_budget";

pub const ACADEMIC_RICH_BUDGET: &str = "academic_rich_budget";

pub const ACADEMIC_RICH_LINK: &str = "academic_rich_link";

pub const ACADEMIC_RICH_SPACING: &str = "academic_rich_spacing";

pub const ACADEMIC_RICH_UNSUPPORTED: &str = "academic_rich_unsupported";

pub const PUBLICATION_SECTION_INVALID: &str = "publication_section_invalid";
pub const PUBLICATION_SECTION_BUDGET: &str = "publication_section_budget";
pub const PUBLICATION_SECTION_GEOMETRY: &str = "publication_section_geometry";
pub const PUBLICATION_SECTION_CONTINUOUS_PAGE: &str = "publication_section_continuous_page";
pub const PUBLICATION_SECTIONS_REQUIRE_PAGED_ENGINE: &str = "publication_sections_require_paged_engine";

pub const ACADEMIC_SOURCE_NOTE: &str = "academic_source_note";

pub const ACADEMIC_SOURCE_OVERLAP: &str = "academic_source_overlap";

pub const ACADEMIC_SOURCE_RANGE: &str = "academic_source_range";

pub const ACADEMIC_SOURCE_REVISION: &str = "academic_source_revision";

pub const ACADEMIC_PROJECTION_SLOTS: &str = "academic_projection_slots";

pub const ACADEMIC_TEX_CONTEXT: &str = "academic_tex_context";

pub const WARN_PUBLICATION_PDF_LIGHT_UNSUPPORTED: &str = "publication_pdf_light_unsupported";

pub const WARN_PUBLICATION_PDF_ITALIC_FONT_UNAVAILABLE: &str = "publication_pdf_italic_font_unavailable";

pub const WARN_PUBLICATION_PDF_SMALL_CAPS_SYNTHESIZED: &str = "publication_pdf_small_caps_synthesized";

// ---- note_code -------------------------------------------------------------
pub const CONVERT_NO_TEXT: &str = "convert_no_text";
pub const OCR_NO_TEXT: &str = "ocr_no_text";
pub const TRANSCRIBE_UNAVAILABLE: &str = crate::transcribe::TRANSCRIBE_UNAVAILABLE;

// ---- warn_items[].code -----------------------------------------------------
pub const WARN_IMAGE_MISSING: &str = "image_missing";
pub const WARN_IMAGE_REMOTE: &str = "image_remote";
pub const WARN_IMAGE_UNSUPPORTED: &str = "image_unsupported";
pub const WARN_FONT_FALLBACK: &str = "font_fallback";
pub const WARN_GLYPH_MISSING: &str = "glyph_missing";
pub const WARN_FORMULA_FALLBACK: &str = "formula_fallback";
pub const WARN_ASSET_COPY_FAILED: &str = "asset_copy_failed";
pub const WARN_OTHER: &str = "other";
pub const WARN_ACADEMIC_MISSING_REFERENCE: &str = "academic_missing_reference";
pub const WARN_PDF_ANCHOR_MISSING: &str = "pdf_anchor_missing";
pub const WARN_ACADEMIC_MISSING_TITLE: &str = "academic_missing_title";
pub const WARN_ACADEMIC_MISSING_AUTHOR: &str = "academic_missing_author";
pub const WARN_ACADEMIC_PROJECTION: &str = "academic_style_projection_limited";
pub const WARN_MATH_DOCUMENT_DEFAULTS: &str = "math_document_defaults_conflict";
pub const WARN_MATH_PARAGRAPH_STYLE_MATERIALIZED: &str = "math_paragraph_style_materialized";
pub const WARN_MATH_RUN_STYLE_MATERIALIZED: &str = "math_run_style_materialized";
pub const WARN_MATHML_ALIGNMENT: &str = "math_manual_alignment_fallback";
pub const WARN_MATHML_SCRIPT_SPACING: &str = "math_script_spacing_fallback";
pub const WARN_MATH_EQARR_INVALID_GEOMETRY: &str = "math_equation_array_invalid_geometry";
pub const WARN_MATH_EQARR_LITERAL_ALIGNMENT: &str = "math_equation_array_literal_alignment";
pub const WARN_MATH_EQARR_MANUAL_SCOPE: &str = "math_equation_array_manual_alignment_scope";
pub const WARN_MATH_EQARR_MARKER_BUDGET: &str = "math_equation_array_marker_budget";
pub const WARN_MATH_EQARR_MATHML_DISTRIBUTION: &str = "math_equation_array_mathml_distribution";
pub const WARN_MATH_EQARR_MIXED_ALIGNMENT: &str = "math_equation_array_mixed_alignment";
pub const WARN_MATH_EQARR_OVERFLOW: &str = "math_equation_array_overflow";
pub const WARN_MATH_EQARR_PARAGRAPH_OVERFLOW: &str = "math_equation_array_paragraph_alignment_overflow";
pub const WARN_MATH_EQARR_SINGLE_COLUMN: &str = "math_equation_array_single_column_distribution";
pub const WARN_MATH_EQARR_WIDTH_UNAVAILABLE: &str = "math_equation_array_width_unavailable";

const EQUATION_ARRAY_WARNINGS: &[&str] = &[
    WARN_MATH_EQARR_INVALID_GEOMETRY,WARN_MATH_EQARR_LITERAL_ALIGNMENT,
    WARN_MATH_EQARR_MANUAL_SCOPE,WARN_MATH_EQARR_MARKER_BUDGET,
    WARN_MATH_EQARR_MATHML_DISTRIBUTION,WARN_MATH_EQARR_MIXED_ALIGNMENT,
    WARN_MATH_EQARR_OVERFLOW,WARN_MATH_EQARR_PARAGRAPH_OVERFLOW,
    WARN_MATH_EQARR_SINGLE_COLUMN,WARN_MATH_EQARR_WIDTH_UNAVAILABLE,
];

/// Every code the frontend must be able to localise (checked by
/// `tools/check-i18n.mjs`).
pub const ERROR_CODES: &[&str] = &[
    FILE_NOT_FOUND, CONVERSION_FAILED, UNSUPPORTED_FORMAT, LEGACY_OFFICE_PARSE_FAILED, OCR_FAILED,OCR_BUSY,OCR_TIMEOUT,
    OCR_NO_ENGINE, ENCODING_UNREPRESENTABLE, ENCODING_UNKNOWN, DIALOG_UNAVAILABLE, PATH_NOT_FOUND,
    OPEN_FAILED, PRESET_NAME_CONFLICT, INTERNAL_ERROR, ZIP_CORRUPT, ZIP_TOO_LARGE, ZIP_UNSUPPORTED,
    CANCELLED,
    DOCX_CITATION_INVALID,DOCX_CITATION_CONFLICT,DOCX_CITATION_RESOURCES_INVALID,
    PUBLICATION_NUMBERING_INVALID,PUBLICATION_NUMBERING_BUDGET,PUBLICATION_NUMBERING_HIERARCHY,PUBLICATION_NUMBERING_CHAPTER_MISSING,
    DOCX_NUMBERING_INVALID,DOCX_NUMBERING_BUDGET,DOCX_NUMBERING_UNREPRESENTABLE,DOCX_NUMBERING_IMPORT_CONFLICT,
    PUBLICATION_NOTE_POLICY_INVALID,PUBLICATION_NOTE_POLICY_UNSUPPORTED,PUBLICATION_NOTE_BINDING_MISMATCH,
    DOCX_NOTE_SETTINGS_INVALID,DOCX_NOTE_SETTINGS_UNSUPPORTED,
    AUTHOR_INLINE_TEXT_INVALID,AUTHOR_INLINE_TEXT_UNSUPPORTED,AUTHOR_INLINE_TEXT_TARGET,AUTHOR_INLINE_TEXT_BUDGET,AUTHOR_INLINE_TEXT_PDF_SHAPING_UNSUPPORTED,
    OPENAPI_INVALID,OPENAPI_BUDGET,OPENAPI_BUSY,OPENAPI_VERSION,OPENAPI_REGION_INVALID,OPENAPI_LOCAL_CHANGES,
    LATEX_ENGINE_UNAVAILABLE,LATEX_INVALID_ENGINE,LATEX_INVALID_OPTIONS,LATEX_WORKSPACE_FAILED,
    LATEX_TIMEOUT,LATEX_OUTPUT_LIMIT,LATEX_PROCESS_FAILED,LATEX_INVALID_OUTPUT,
    LATEX_COMPILATION_FAILED,LATEX_REFERENCES_UNSETTLED,LATEX_SAVE_FAILED,LATEX_BUSY,
    TYPST_ENGINE_UNAVAILABLE,TYPST_WORKSPACE_FAILED,TYPST_TIMEOUT,TYPST_OUTPUT_LIMIT,TYPST_PROCESS_FAILED,TYPST_INVALID_OUTPUT,TYPST_COMPILATION_FAILED,TYPST_SAVE_FAILED,TYPST_BUSY,TYPST_FORMULA_FAILED,TYPST_INVALID_CONTENT,
    PUBLICATION_LAYOUT_GEOMETRY,PUBLICATION_NOTE_LAYOUT,PUBLICATION_FONTS_UNAVAILABLE,PUBLICATION_RUNNING_OVERLAP,
    PUBLICATION_LINE_SPACING_UNSUPPORTED,
    PUBLICATION_TRACKING_UNSUPPORTED,
    PUBLICATION_BASELINE_UNSUPPORTED,EPUB_INVALID_OPTIONS,MATH_METADATA_STALE,MATH_METADATA_INVALID,MATH_DOCUMENT_DEFAULTS_CONFLICT,
    PUBLICATION_LOCAL_INVALID,PUBLICATION_LOCAL_VERSION,PUBLICATION_LOCAL_ORPHAN,PUBLICATION_LOCAL_DUPLICATE,PUBLICATION_LOCAL_TARGET,DOCX_NESTED_FOOTNOTE,DOCX_NOTES_INVALID,
    PDF_BUSY,PDF_CONFLICT,PDF_ENGINE_UNAVAILABLE,PDF_INVALID_OPERATION,PDF_INVALID_PAGE,
    PDF_LAST_PAGE,PDF_LIMIT,PDF_MISSING_GLYPH,PDF_OPEN_FAILED,PDF_OPERATION_FAILED,
    PDF_READ_ONLY,PDF_SAVE_FAILED,PDF_SESSION_EXPIRED,PDF_SIGNED,PDF_TEXT_OVERFLOW,
    PDF_TIMEOUT,PDF_UNSUPPORTED_OBJECT,PDF_WORKER_FAILED,CAPABILITY_PACK_INVALID,
    PUBLICATION_METADATA_ACTION,
    PUBLICATION_METADATA_ALIAS_CONFLICT,
    PUBLICATION_METADATA_BUDGET,
    PUBLICATION_METADATA_CLEAR_CONFLICT,
    PUBLICATION_METADATA_DUPLICATE,
    PUBLICATION_METADATA_DUPLICATE_ID,
    PUBLICATION_METADATA_FIELD,
    PUBLICATION_METADATA_ID,
    PUBLICATION_METADATA_IDENTIFIER_ROLE,
    PUBLICATION_METADATA_ISBN,
    PUBLICATION_METADATA_LANGUAGE,
    PUBLICATION_METADATA_ORCID,
    PUBLICATION_METADATA_PATCH_SYNTAX,
    PUBLICATION_METADATA_PRIMARY,
    PUBLICATION_METADATA_REFERENCE,
    PUBLICATION_METADATA_REQUIRED,
    PUBLICATION_METADATA_TYPE,
    PUBLICATION_METADATA_YAML,
    PUBLICATION_METADATA_YAML_FEATURE,
    PUBLICATION_METADATA_PDF,
    ACADEMIC_CONTEXT,
    ACADEMIC_CONTEXT_BUDGET,
    ACADEMIC_RICH_BUDGET,
    ACADEMIC_RICH_LINK,
    ACADEMIC_RICH_SPACING,
    ACADEMIC_RICH_UNSUPPORTED,
    PUBLICATION_SECTION_INVALID,
    PUBLICATION_SECTION_BUDGET,
    PUBLICATION_SECTION_GEOMETRY,
    PUBLICATION_SECTION_CONTINUOUS_PAGE,
    PUBLICATION_SECTIONS_REQUIRE_PAGED_ENGINE,
    ACADEMIC_SOURCE_NOTE,
    ACADEMIC_SOURCE_OVERLAP,
    ACADEMIC_SOURCE_RANGE,
    ACADEMIC_SOURCE_REVISION,
    ACADEMIC_PROJECTION_SLOTS,
    ACADEMIC_TEX_CONTEXT,
    PUBLICATION_PDF_RICH_FORMAT_INVALID,
    ACADEMIC_AFFIX_UNSUPPORTED,
];
pub const NOTE_CODES: &[&str] = &[CONVERT_NO_TEXT, OCR_NO_TEXT, TRANSCRIBE_UNAVAILABLE];
pub const WARN_CODES: &[&str] = &[
    WARN_DOCX_CITATION_INVALID,WARN_DOCX_CITATION_CONFLICT,WARN_DOCX_CITATION_RESOURCES_INVALID,
    WARN_PUBLICATION_NUMBERING_TAB,WARN_PUBLICATION_NOTE_PAGE_RESTART,WARN_PUBLICATION_NOTE_SECTION_RESTART,WARN_PUBLICATION_NOTE_CONTINUATION,
    WARN_ACADEMIC_UNPRINTABLE_REFERENCE,WARN_ACADEMIC_DIAGNOSTIC,
    WARN_PUBLICATION_METADATA_DATE_CUSTOM_ONLY,WARN_PUBLICATION_METADATA_SERIES_POSITION_CUSTOM_ONLY,
    WARN_MATH_DOCUMENT_DEFAULTS,
    WARN_MATH_PARAGRAPH_STYLE_MATERIALIZED,
    WARN_MATH_RUN_STYLE_MATERIALIZED,
    WARN_MATHML_ALIGNMENT,
    WARN_MATHML_SCRIPT_SPACING,
    WARN_MATH_EQARR_INVALID_GEOMETRY,WARN_MATH_EQARR_LITERAL_ALIGNMENT,
    WARN_MATH_EQARR_MANUAL_SCOPE,WARN_MATH_EQARR_MARKER_BUDGET,
    WARN_MATH_EQARR_MATHML_DISTRIBUTION,WARN_MATH_EQARR_MIXED_ALIGNMENT,
    WARN_MATH_EQARR_OVERFLOW,WARN_MATH_EQARR_PARAGRAPH_OVERFLOW,
    WARN_MATH_EQARR_SINGLE_COLUMN,WARN_MATH_EQARR_WIDTH_UNAVAILABLE,
    WARN_IMAGE_MISSING, WARN_IMAGE_REMOTE, WARN_IMAGE_UNSUPPORTED, WARN_FONT_FALLBACK,
    WARN_GLYPH_MISSING, WARN_FORMULA_FALLBACK, WARN_ASSET_COPY_FAILED,
    WARN_ACADEMIC_MISSING_REFERENCE,WARN_ACADEMIC_MISSING_TITLE,WARN_ACADEMIC_MISSING_AUTHOR,WARN_ACADEMIC_PROJECTION,
    WARN_PDF_ANCHOR_MISSING,
    WARN_PUBLICATION_LOCAL_UNSUPPORTED,
    WARN_PUBLICATION_LAYOUT_PAGED,WARN_PUBLICATION_LAYOUT_PARAGRAPH,WARN_PUBLICATION_LAYOUT_NOTES,WARN_PUBLICATION_LAYOUT_HEADING,
    WARN_PUBLICATION_CONTINUOUS_COLUMNS_PARTIAL,
    WARN_PUBLICATION_FONT_SUBSTITUTED,
    WARN_PUBLICATION_METADATA_TITLE_FALLBACK,
    WARN_PUBLICATION_METADATA_LABEL_FALLBACK,
    WARN_PUBLICATION_PDF_LIGHT_UNSUPPORTED,
    WARN_PUBLICATION_PDF_ITALIC_FONT_UNAVAILABLE,
    WARN_PUBLICATION_PDF_SMALL_CAPS_SYNTHESIZED,
];

/// Prefix → (code, name of the single `{param}`).  The warning producers keep
/// their historical Chinese text; this table is the one place that knows
/// their shapes.  Longer prefixes first.
const WARN_PREFIXES: &[(&str, &str, &str)] = &[
    ("docx_citation_invalid",WARN_DOCX_CITATION_INVALID,""),
    ("docx_citation_conflict",WARN_DOCX_CITATION_CONFLICT,""),
    ("docx_citation_resources_invalid",WARN_DOCX_CITATION_RESOURCES_INVALID,""),
    ("publication_numbering_tab_unsupported",WARN_PUBLICATION_NUMBERING_TAB,""),
    ("publication_note_page_restart_unsupported",WARN_PUBLICATION_NOTE_PAGE_RESTART,""),
    ("publication_note_section_restart_unsupported",WARN_PUBLICATION_NOTE_SECTION_RESTART,""),
    ("publication_note_continuation_unsupported",WARN_PUBLICATION_NOTE_CONTINUATION,""),
    ("academic_unprintable_reference: ",WARN_ACADEMIC_UNPRINTABLE_REFERENCE,"key"),
    ("academic_diagnostic: ",WARN_ACADEMIC_DIAGNOSTIC,"key"),
    ("publication_metadata_date_custom_only",WARN_PUBLICATION_METADATA_DATE_CUSTOM_ONLY,""),
    ("publication_metadata_series_position_custom_only",WARN_PUBLICATION_METADATA_SERIES_POSITION_CUSTOM_ONLY,""),
    ("publication_pdf_small_caps_synthesized",WARN_PUBLICATION_PDF_SMALL_CAPS_SYNTHESIZED,""),
    ("publication_pdf_italic_font_unavailable",WARN_PUBLICATION_PDF_ITALIC_FONT_UNAVAILABLE,""),
    ("publication_pdf_light_unsupported",WARN_PUBLICATION_PDF_LIGHT_UNSUPPORTED,""),
    ("publication_metadata_label_fallback",WARN_PUBLICATION_METADATA_LABEL_FALLBACK,""),
    ("publication_metadata_title_fallback",WARN_PUBLICATION_METADATA_TITLE_FALLBACK,""),
    ("math_document_defaults_conflict",WARN_MATH_DOCUMENT_DEFAULTS,""),
    ("math_manual_alignment_fallback",WARN_MATHML_ALIGNMENT,""),
    ("math_script_spacing_fallback",WARN_MATHML_SCRIPT_SPACING,""),
    ("publication_local_style_unsupported",WARN_PUBLICATION_LOCAL_UNSUPPORTED,""),
    ("publication_layout_paged_unsupported",WARN_PUBLICATION_LAYOUT_PAGED,""),
    ("publication_layout_paragraph_unsupported",WARN_PUBLICATION_LAYOUT_PARAGRAPH,""),
    ("publication_layout_notes_unsupported",WARN_PUBLICATION_LAYOUT_NOTES,""),
    ("publication_layout_heading_unsupported",WARN_PUBLICATION_LAYOUT_HEADING,""),
    ("publication_continuous_columns_partial",WARN_PUBLICATION_CONTINUOUS_COLUMNS_PARTIAL,""),
    ("publication_font_substituted",WARN_PUBLICATION_FONT_SUBSTITUTED,""),
    ("academic_missing_reference: ", WARN_ACADEMIC_MISSING_REFERENCE, "key"),
    ("pdf_anchor_missing: ", WARN_PDF_ANCHOR_MISSING, "key"),
    ("academic_missing_title: ", WARN_ACADEMIC_MISSING_TITLE, "key"),
    ("academic_missing_author: ", WARN_ACADEMIC_MISSING_AUTHOR, "key"),
    ("academic_style_projection_limited", WARN_ACADEMIC_PROJECTION, ""),
    ("远程/内联图片不支持嵌入，已跳过：", WARN_IMAGE_REMOTE, "src"),
    ("图片不存在，已跳过：", WARN_IMAGE_MISSING, "src"),
    ("图片无法嵌入：", WARN_IMAGE_UNSUPPORTED, "src"),
    ("EPUB 不支持该图片格式，已跳过：", WARN_IMAGE_UNSUPPORTED, "src"),
    ("图片读取失败，已跳过：", WARN_IMAGE_MISSING, "src"),
    ("DOCX 不支持该图片格式", WARN_IMAGE_UNSUPPORTED, "src"),
    ("所选字体不可用，已使用中文后备字体：", WARN_FONT_FALLBACK, "font"),
    ("font_fallback: ", WARN_FONT_FALLBACK, "font"),
    ("未找到可嵌入的 CJK 字体", WARN_FONT_FALLBACK, ""),
    ("嵌入字体缺少以下字符，已使用阅读器内置字体：", WARN_GLYPH_MISSING, "chars"),
    ("公式无法渲染，已按文本保留：", WARN_FORMULA_FALLBACK, "latex"),
    ("资源复制失败：", WARN_ASSET_COPY_FAILED, "name"),
];

/// Classify one legacy warning string.
pub fn warn_item(text: &str) -> Value {
    // Exact machine codes only. A lookalike suffix must not be accepted as a
    // known warning or interpolate arbitrary source into a translated message.
    if [WARN_MATH_PARAGRAPH_STYLE_MATERIALIZED,WARN_MATH_RUN_STYLE_MATERIALIZED].contains(&text) || EQUATION_ARRAY_WARNINGS.contains(&text) {return json!({"code":text,"params":{},"text":text});}
    for (prefix, code, param) in WARN_PREFIXES {
        if let Some(rest) = text.strip_prefix(prefix) {
            let mut params = serde_json::Map::new();
            if !param.is_empty() {
                // `DOCX 不支持该图片格式（png），已跳过：src` keeps the tail after the last `：`.
                let value = if *code == WARN_IMAGE_UNSUPPORTED && prefix.starts_with("DOCX") {
                    rest.rsplit_once('：').map(|(_, s)| s).unwrap_or(rest)
                } else {
                    rest
                };
                params.insert((*param).to_string(), json!(value));
            }
            return json!({ "code": code, "params": params, "text": text });
        }
    }
    json!({ "code": WARN_OTHER, "params": {}, "text": text })
}

/// `warns: [String]` → `warn_items: [{code, params, text}]`.
pub fn warn_items<S: AsRef<str>>(warns: &[S]) -> Value {
    Value::Array(warns.iter().map(|w| warn_item(w.as_ref())).collect())
}

/// Add `warn_items` next to an existing `warns` string array (no-op otherwise).
pub fn attach_warn_items(body: &mut Value) {
    let Some(map) = body.as_object_mut() else { return };
    let items = match map.get("warns") {
        Some(Value::Array(list)) => {
            let texts: Vec<&str> = list.iter().filter_map(|v| v.as_str()).collect();
            warn_items(&texts)
        }
        _ => return,
    };
    map.insert("warn_items".to_string(), items);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_paragraph_style_warning_is_typed_exactly_through_both_warning_envelopes() {
        let code=WARN_MATH_PARAGRAPH_STYLE_MATERIALIZED;
        assert!(WARN_CODES.contains(&code));
        assert_eq!(warn_item(code),json!({"code":code,"params":{},"text":code}));
        assert_eq!(warn_items(&[code])[0]["code"],code);
        let mut body=json!({"warns":[code]});attach_warn_items(&mut body);
        assert_eq!(body["warn_items"][0]["code"],code);
        for lookalike in [format!("{code}_lookalike"),format!("{code}/private.docx"),format!("{code}: source"),format!(" {code}")] {
            assert_eq!(warn_item(&lookalike)["code"],WARN_OTHER);
        }
    }

    #[test]
    fn native_run_style_warning_is_exact_and_survives_warning_envelopes() {
        let code=WARN_MATH_RUN_STYLE_MATERIALIZED;
        assert!(WARN_CODES.contains(&code));
        assert_eq!(warn_item(code),json!({"code":code,"params":{},"text":code}));
        assert_eq!(warn_items(&[code])[0]["code"],code);
        let mut body=json!({"warns":[code]});attach_warn_items(&mut body);
        assert_eq!(body["warn_items"][0],warn_item(code));
        for lookalike in [format!("{code}_lookalike"),format!("{code}/private.docx"),format!("{code}: source"),format!(" {code}")] {
            assert_eq!(warn_item(&lookalike)["code"],WARN_OTHER);
        }
    }

    #[test]
    fn equation_array_warning_codes_are_registered_and_match_exactly() {
        for code in EQUATION_ARRAY_WARNINGS {
            assert!(WARN_CODES.contains(code));assert_eq!(warn_item(code)["code"],*code);
            assert_eq!(warn_item(&format!("{code}_lookalike"))["code"],WARN_OTHER);
        }
    }

    #[test]
    fn every_known_producer_prefix_maps_to_a_declared_code() {
        for (_, code, _) in WARN_PREFIXES {
            assert!(WARN_CODES.contains(code), "{code}");
        }
        let cases = [
            ("图片不存在，已跳过：a.png", WARN_IMAGE_MISSING, Some(("src", "a.png"))),
            ("远程/内联图片不支持嵌入，已跳过：https://x/y.png", WARN_IMAGE_REMOTE, Some(("src", "https://x/y.png"))),
            ("DOCX 不支持该图片格式（svg），已跳过：a.svg", WARN_IMAGE_UNSUPPORTED, Some(("src", "a.svg"))),
            ("公式无法渲染，已按文本保留：a+b", WARN_FORMULA_FALLBACK, Some(("latex", "a+b"))),
            ("嵌入字体缺少以下字符，已使用阅读器内置字体：𠀀", WARN_GLYPH_MISSING, Some(("chars", "𠀀"))),
            ("未找到可嵌入的 CJK 字体，非拉丁文字…", WARN_FONT_FALLBACK, None),
            ("publication_font_substituted", WARN_PUBLICATION_FONT_SUBSTITUTED, None),
            ("publication_continuous_columns_partial", WARN_PUBLICATION_CONTINUOUS_COLUMNS_PARTIAL, None),
            ("something new", WARN_OTHER, None),
        ];
        for (text, code, param) in cases {
            let v = warn_item(text);
            assert_eq!(v["code"], code, "{text}");
            assert_eq!(v["text"], text);
            if let Some((k, want)) = param {
                assert_eq!(v["params"][k], want, "{text}");
            }
        }
    }

    #[test]
    fn attach_keeps_legacy_warns_and_adds_items() {
        let mut body = json!({ "ok": true, "warns": ["图片不存在，已跳过：x.png"] });
        attach_warn_items(&mut body);
        assert_eq!(body["warns"][0], "图片不存在，已跳过：x.png");
        assert_eq!(body["warn_items"][0]["code"], WARN_IMAGE_MISSING);
        let mut none = json!({ "ok": false });
        attach_warn_items(&mut none);
        assert!(none.get("warn_items").is_none());
    }
}
