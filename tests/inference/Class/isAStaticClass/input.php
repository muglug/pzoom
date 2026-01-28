<?php
class C {
    public function foo1(): string {
        if (is_a(static::class, D::class, true)) {
            return $this->baz();
        }
        return "";
    }

    public static function foo2(): string {
        if (is_a(static::class, D::class, true)) {
            return static::bat();
        }
        return "";
    }
}

class D extends C {
    public function baz(): string {
        return "baz";
    }

    public static function bat(): string {
        return "baz";
    }
}
