<?php
/**
 * @psalm-param list<string> $elements
 * @return list<string>
 */
function bar(array $elements, int $index, string $element) : array {
    array_splice($elements, $index, 0, [$element]);
    return $elements;
}
