<?php
interface Foo {
    public static function foo(): ?string;
}

class Bar implements Foo {
    public static function foo(): ?string
    {
        return "bar";
    }
}

class Baz implements Foo {
    /**
     * @return string $baz
     */
    public static function foo(): ?string
    {
        return "baz";
    }
}

class Bax implements Foo {
    /**
     * @return null|string $baz
     */
    public static function foo(): ?string
    {
        return "bax";
    }
}

class Baw implements Foo {
    /**
     * @return null|string $baz
     */
    public static function foo(): ?string
    {
        /** @var null|string $val */
        $val = "baw";

        return $val;
    }
}
