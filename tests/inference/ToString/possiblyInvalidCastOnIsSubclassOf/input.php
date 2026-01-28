<?php
class Foo {}

/**
 * @param mixed $a
 */
function bar($a) : ?string {
    /**
     * @psalm-suppress MixedArgument
     */
    if (is_subclass_of($a, Foo::class)) {
        return "hello" . $a;
    }

    return null;
}
