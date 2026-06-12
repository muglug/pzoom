<?php
namespace Barrr;

/**
 * @psalm-type aType null|"a"|"b"|"c"|"d"
 */

/** @psalm-return array{0:bool,1:aType} */
function f(): array {
    return [(bool)rand(0,1), rand(0,1) ? "z" : null];
}
