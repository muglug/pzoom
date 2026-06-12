<?php
class Foo {
    const BOOL_VAR_EXPORT_RETURN = true;

    /**
     * @param mixed $mixed
     */
    public static function Baz($mixed) : string {
        return var_export($mixed, self::BOOL_VAR_EXPORT_RETURN);
    }
}
