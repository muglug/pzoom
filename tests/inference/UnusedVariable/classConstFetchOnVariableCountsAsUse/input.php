<?php

class IssueA { public const SHORTCODE = 1; }
class IssueB { public const SHORTCODE = 2; }

/** @return array<int, list<string>> */
function collectShortcodes(): array
{
    $all_issues = ['IssueA', 'IssueB'];
    $all_shortcodes = [];

    foreach ($all_issues as $issue_type) {
        /** @var class-string $issue_class */
        $issue_class = '\\' . $issue_type;
        /** @var int $shortcode */
        $shortcode = $issue_class::SHORTCODE;
        $all_shortcodes[$shortcode][] = $issue_type;
    }
    return $all_shortcodes;
}
