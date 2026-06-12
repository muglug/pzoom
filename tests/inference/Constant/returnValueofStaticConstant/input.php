<?php
class Foo
{
    public const BAR = ["bar"];

    /**
     * @return value-of<static::BAR>
     */
    public function bar(): string
    {
        return static::BAR[0];
    }
}
