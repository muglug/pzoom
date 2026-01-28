<?php
class C {
    public function work(object $obj): string {
        if (get_class($obj) === self::class) {
            return $obj->baz();
        }
        return "";
    }

    public function baz(): string {
        return "baz";
    }
}
