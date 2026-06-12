<?php
final class MethodId { public function __construct(public string $name) {} }

function f(MethodId $method_id, ?string $source_method_id, bool $collect): void {
    $loc = $collect && $method_id !== $source_method_id ? 'yes' : null;
    echo (string) $loc;
}
