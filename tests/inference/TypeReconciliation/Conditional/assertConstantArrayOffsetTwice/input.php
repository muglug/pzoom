<?php
class A {
    const FOO = "foo";
    const BAR = "bar";

    /** @psalm-suppress MixedArgument */
    public function bar(array $args) : void {
        if ($args[self::FOO]) {
            echo $args[self::FOO];
        }
        if ($args[self::BAR]) {
            echo $args[self::BAR];
        }
    }
}