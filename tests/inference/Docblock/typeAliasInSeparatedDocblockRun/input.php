<?php
class IssueData {}

/**
 * @psalm-type TaggedCodeType = array<int, array{0: int, 1: non-empty-string}>
 *
 * @psalm-type WorkerData = array{
 *      issues: array<string, list<IssueData>>,
 *      fixable_issue_counts: array<string, int>,
 * }
 */

/**
 * @internal
 *
 * A second docblock between the alias block and the class.
 */
final class Analyzer {}

/**
 * @psalm-import-type WorkerData from Analyzer
 */
final class Probe {
    /** @param WorkerData $wd */
    public function probe(array $wd): void {
        takesShape($wd['issues']);
    }
}

/** @param array<string, list<IssueData>> $issues */
function takesShape(array $issues): void {}
