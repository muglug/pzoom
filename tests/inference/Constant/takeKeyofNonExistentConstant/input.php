<?php
class Foo
{
    public const BAR = ["bar"];

    /**
     * @param key-of<self::BAT> $key
     */
    public function bar(int $key): string
    {
        return static::BAR[$key];
    }
}
