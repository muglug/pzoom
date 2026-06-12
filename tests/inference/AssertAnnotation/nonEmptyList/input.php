<?php
/**
 * @psalm-assert non-empty-list $array
 *
 * @param mixed  $array
 */
function isNonEmptyList($array): void {}

/**
 * @psalm-param mixed $value
 *
 * @psalm-return non-empty-list<mixed>
 */
function consume1($value): array {
    isNonEmptyList($value);
    return $value;
}

/**
 * @psalm-param list<string> $values
 */
function consume2(array $values): void {
    isNonEmptyList($values);
    foreach ($values as $str) {}
    echo $str;
}
