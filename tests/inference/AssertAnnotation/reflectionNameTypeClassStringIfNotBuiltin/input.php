<?php
/** @return class-string|null */
function getPropertyType(\ReflectionProperty $reflectionItem): ?string {
    $type = $reflectionItem->getType();
    return ($type instanceof \ReflectionNamedType) && !$type->isBuiltin() ? $type->getName() : null;
}
