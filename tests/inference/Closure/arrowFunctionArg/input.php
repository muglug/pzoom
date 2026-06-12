<?php
/** @var array<string,array{o:int, s:array<int, string>}> $existingIssue */
array_reduce(
    $existingIssue,
    /**
     * @param array{o:int, s:array<int, string>} $existingIssue
     */
    static fn(int $carry, array $existingIssue): int => $carry + $existingIssue["o"],
    0,
);
