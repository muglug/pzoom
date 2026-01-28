<?php
function getTypeName(ReflectionParameter $parameter): string {
    $type = $parameter->getType();

    if ($type === null) {
        return "mixed";
    }

    if ($type instanceof ReflectionUnionType) {
        return "union";
    }

    if ($type instanceof ReflectionNamedType) {
        return $type->getName();
    }

    throw new RuntimeException("unexpected type");
}
