<?php
final class Foo {
    public const VALUE_ANY = null;
    public const VALUE_ONE = "one";

    /** @return self::VALUE_* */
    public static function getValues() {
        return rand(0, 1) ? null : self::VALUE_ONE;
    }
}

$data = Foo::getValues();

if ($data === Foo::VALUE_ANY) {
    $data = "default";
}

echo strlen($data);