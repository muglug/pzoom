<?php
class Foo
{
    /** @var string */
    public const BAR = "bar";

    /**
     * @return "bar"
     */
    public function bar(): string
    {
        return self::BAR;
    }
}
