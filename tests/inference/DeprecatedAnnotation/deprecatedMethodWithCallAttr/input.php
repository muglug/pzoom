<?php
class Foo {
    #[\Deprecated]
    public static function barBar(): void {
    }
}

Foo::barBar();
