<?php
/**
 * @psalm-assert non-empty-list<string> $array
 *
 * @param mixed  $array
 */
function isNonEmptyListOfStrings($array): void {}

/**
 * @psalm-param list<string> $values
 */
function consume2(array $values): void {
    isNonEmptyListOfStrings($values);
    foreach ($values as $str) {}
    echo $str;
}
