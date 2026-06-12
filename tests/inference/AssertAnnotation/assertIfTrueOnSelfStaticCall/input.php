<?php
abstract class Atomic2 {}
final class TNamed extends Atomic2 { public ?string $extra_types = null; }

final class TypeHelper {
    /**
     * @psalm-assert-if-true TNamed $type
     */
    public static function isIntersectionType(Atomic2 $type): bool {
        return $type instanceof TNamed;
    }

    private static function hasIntersection(Atomic2 $type): bool {
        return self::isIntersectionType($type) && $type->extra_types !== null;
    }

    public static function go(Atomic2 $type): bool { return self::hasIntersection($type); }
}
