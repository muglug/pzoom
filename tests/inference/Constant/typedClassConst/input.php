<?php
class Foo
{
    /** @var string */
    public const BAR = "bar";

    public function bar(): string
    {
        return static::BAR;
    }
}
