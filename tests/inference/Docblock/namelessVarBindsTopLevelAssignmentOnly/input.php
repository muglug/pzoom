<?php
final class TArr {}
final class TKeyed {}
final class Prov { public function getType(string $k): ?string { return $k === '' ? null : $k; } }

function f(Prov $p, string $key): void {
    /**
     * @var TArr|TKeyed|null
     */
    $array_arg_type = ($arg_value_type = $p->getType($key))
            && $arg_value_type !== 'no'
        ? new TArr()
        : null;
    if ($array_arg_type instanceof TArr) { echo 'a'; }
    if ($arg_value_type !== null) { echo strlen($arg_value_type); }
}
