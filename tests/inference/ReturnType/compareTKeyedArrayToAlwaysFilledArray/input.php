<?php
/**
 * @param array<"from"|"to", bool> $a
 *
 * This is unsealed because there is no way to mark a TArray as sealed.
 *
 * @return array{from: bool, to: bool}
 */
function foo(array $a) : array {
    return $a;
}
