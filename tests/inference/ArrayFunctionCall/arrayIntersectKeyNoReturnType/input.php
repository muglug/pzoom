<?php
/**
 * @psalm-suppress MissingReturnType
 */
function unknown() {
    return ["x" => "hello"];
}

class C {
    /**
     * @psalm-suppress MissingReturnType
     */
    public static function unknownStatic() {
        return ["x" => "hello"];
    }

    /**
     * @psalm-suppress MissingReturnType
     */
    public static function unknownInstance() {
        return ["x" => "hello"];
    }
}

/**
 * @psalm-suppress MixedArgument
 */
function sdn(array $s) : void {
    $r = array_intersect_key(unknown(), array_filter($s));
    if (empty($r)) {}

    $r = array_intersect_key(C::unknownStatic(), array_filter($s));
    if (empty($r)) {}

    $r = array_intersect_key((new C)->unknownInstance(), array_filter($s));
    if (empty($r)) {}
}
