<?php
/** @psalm-immutable */
class TC {
    public ?string $return_type = null;
}

/**
 * @psalm-suppress InaccessibleProperty Allowed during construction
 */
final class TP {
    public static function build(?string $rt): TC {
        $callable_type = new TC();
        $callable_type->return_type = $rt;
        return $callable_type;
    }
}
