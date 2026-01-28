<?php
/**
 * @param array<string, array{x?:int, y?:int, width?:int, height?:int}> $foos
 */
function foo(array $foos): void {
    array_multisort(array_column($foos, "y"), SORT_ASC, $foos);
}
