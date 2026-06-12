<?php

abstract class TypeAtomic {
    abstract public function toPhpString(int $version): ?string;
}
abstract class TypeScalar extends TypeAtomic {}
class TypeObject extends TypeAtomic {
    public function toPhpString(int $version): ?string { return $version >= 1 ? 'object' : null; }
}
class TypeArray extends TypeAtomic {
    public function toPhpString(int $version): string { return 'array'; }
}
class TypeIterable extends TypeArray {
}

function convertToIdentifier(TypeAtomic $atomic_type): string {
    if ($atomic_type instanceof TypeScalar
        || $atomic_type instanceof TypeObject
        || $atomic_type instanceof TypeArray
        || $atomic_type instanceof TypeIterable
    ) {
        $identifier_string = $atomic_type->toPhpString(80000);

        if ($identifier_string === null) {
            return 'unknown';
        }
        return $identifier_string;
    }
    return 'other';
}
