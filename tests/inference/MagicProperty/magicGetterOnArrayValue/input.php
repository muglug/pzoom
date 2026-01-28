<?php
/**
 * @property string $send_type
 * @psalm-seal-properties
 */
final class b {
    public function __set(string $key, mixed $value) {}
}

/**
 * @param array<string, b> $objs
 */
function test(string $k, array $objs): void {
    $objs[$k]->send_type = "test";
}
