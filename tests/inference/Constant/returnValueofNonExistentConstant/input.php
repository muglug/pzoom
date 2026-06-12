<?php
class Foo
{
    public const BAR = ["bar"];

    /**
     * @return value-of<self::BAT>
     */
    public function bar(): string
    {
        return self::BAR[0];
    }
}
