<?php
/** @param array<string, bool>|null $class_strings */
function takesClassStrings(?array $class_strings): void {}

/**
 * @param non-empty-array<int|string, true>|null $strings
 * @psalm-suppress InvalidArgument
 */
function f(?array $strings): void {
    takesClassStrings($strings);
}
