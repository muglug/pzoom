<?php
/**
 * @param non-falsy-string $arg
 * @return non-falsy-string
 */
function foo( $arg ) {
    /** @psalm-suppress UndefinedConstant */
    return FOO . $arg;
}
