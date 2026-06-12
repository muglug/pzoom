<?php
/**
 * @psalm-return array{
 *     a: int,
 *     b: string,
 * }
 */
function foo(): array {
    return ["a" => 1, "b" => "two"];
}
