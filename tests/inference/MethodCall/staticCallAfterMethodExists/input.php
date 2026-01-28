<?php
class A
{
    protected static function existing() : string
    {
        return "hello";
    }

    protected static function foo() : string
    {
        if (!method_exists(static::class, "maybeExists")) {
            return "hello";
        }

        self::maybeExists();

        return static::existing();
    }
}
