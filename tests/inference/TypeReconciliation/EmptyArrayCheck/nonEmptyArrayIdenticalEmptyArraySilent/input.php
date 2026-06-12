<?php

/** @param non-empty-array<string, int> $arr */
function checkGenericNonEmpty(array $arr): string {
    if ($arr === []) {
        return 'empty';
    }
    return 'full';
}

/** @param non-empty-list<int> $l */
function checkNonEmptyList(array $l): string {
    if ($l === []) {
        return 'empty';
    }
    return 'full';
}
