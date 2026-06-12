<?php

namespace Some\Deep;

class IssueA {}
class IssueB {}

class Holder {
    private const ISSUES = [
        IssueA::class,
        IssueB::class,
    ];

    /** @return array<int, string> */
    public static function shortNames(): array
    {
        return array_map(
            /** @param class-string $issue_class */
            static function (string $issue_class): string {
                $parts = explode('\\', $issue_class);
                return end($parts) ?: '';
            },
            self::ISSUES,
        );
    }
}
