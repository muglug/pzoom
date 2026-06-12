<?php

class Storage8 {
    /** @var array<string, array<string, string>>|null */
    public ?array $template_extended_params = null;
}

class Base8 {
    public string $value = 'x';
}

function f(Storage8 $classlike_storage, Base8 $base_type): ?string
{
    if (!empty($classlike_storage->template_extended_params[$base_type->value])) {
        $vals = array_values($classlike_storage->template_extended_params[$base_type->value]);
        return $vals[0] ?? null;
    }
    return null;
}
