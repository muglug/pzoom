<?php
interface ToBeIgnored
{
    /**
     * @param mixed $value
     * @return mixed
     */
    public static function of($value);
}

interface ToBeUsed extends ToBeIgnored
{
    /**
     * @template U
     * @param U $value
     * @return U
     */
    public static function of($value);
}

interface ExtendsToBeUsed extends ToBeUsed {}

class Foo implements ExtendsToBeUsed {
    /** @psalm-suppress InvalidReturnType */
    public static function of($value) {}
}

function bar(Foo $f, string $s) : string {
    return $f::of($s);
}