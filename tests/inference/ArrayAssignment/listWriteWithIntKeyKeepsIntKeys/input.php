<?php
final class U7 {}
final class Comb {
    /** @var array<string, non-empty-list<U7>> */
    public array $builtin_type_params = [];
}
function f(Comb $c): void {
    if (isset($c->builtin_type_params['iterable'])) {
        foreach ($c->builtin_type_params['iterable'] as $i => $_p) {
            /** @psalm-suppress PropertyTypeCoercion */
            $c->builtin_type_params['iterable'][$i] = new U7();
        }
    }
}
