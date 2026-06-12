<?php
class TplUnion {}
class AsType2 { public string $value = ''; }
class BaseType2 { public ?AsType2 $as_type = null; }
class Storage2 {
    /** @var array<string, array<string, TplUnion>>|null */
    public ?array $template_extended_params = null;
}

/** @return non-empty-list<TplUnion>|null */
function mapped(Storage2 $classlike_storage, BaseType2 $base_type): ?array {
    if (!$base_type->as_type) {
        return null;
    }
    if (!empty($classlike_storage->template_extended_params[$base_type->as_type->value])) {
        return array_values($classlike_storage->template_extended_params[$base_type->as_type->value]);
    }
    return null;
}
