<?php
class C {
    public function foo1(): string {
        if (static::class === D::class) {
            return $this->baz();
        }
        return "";
    }

    public static function foo2(): string {
        if (static::class === D::class) {
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
