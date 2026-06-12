<?php
/** @param non-empty-array<int|string, string> $props */
function takesNonEmpty(array $props): void {}

/** @param array<int|string, string> $props */
function f(array $props, string $v): void {
    array_unshift($props, $v);
    takesNonEmpty($props);
}

/** @param non-empty-list<string> $l */
function takesNonEmptyList(array $l): void {}

/** @param list<string> $l */
function g(array $l, string $v): void {
    array_push($l, $v);
    takesNonEmptyList($l);
}
