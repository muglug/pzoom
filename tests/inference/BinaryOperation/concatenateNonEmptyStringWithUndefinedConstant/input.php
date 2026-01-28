<?php
/**
 * @param non-empty-string $arg
 * @return non-empty-string
 */
function foo( $arg ) {
    /** @psalm-suppress UndefinedConstant */
    return FOO . $arg;
}
