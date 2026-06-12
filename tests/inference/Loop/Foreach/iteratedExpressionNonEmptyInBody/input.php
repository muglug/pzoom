<?php
class IssueZ { public string $message = ''; }
class StoreZ {
    /** @var array<int, IssueZ> */
    public array $docblock_issues = [];
}

function walk(StoreZ $storage): void {
    foreach ($storage->docblock_issues as $issue) {
        if ($issue->message === 'x') {
            $e = reset($storage->docblock_issues);
            echo $e->message;
        }
    }
}
