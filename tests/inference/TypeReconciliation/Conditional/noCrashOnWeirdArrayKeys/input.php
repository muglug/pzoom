<?php
/**
 * @psalm-suppress MixedPropertyFetch
 * @psalm-suppress MixedArrayOffset
 */
function foo(array $a, array $b) : void {
    if (isset($a[$b[0]->id])) {}
}