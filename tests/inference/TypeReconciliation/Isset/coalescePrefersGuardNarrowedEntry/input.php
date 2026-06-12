<?php
class CC32 { public ?string $addr = null; }
function h(CC32 $c): void {
    $options = getopt('h', ['tcp:']);
    if ($options === false) { exit(1); }
    if (isset($options['tcp']) && !is_string($options['tcp'])) {
        exit(1);
    }
    $c->addr = $options['tcp'] ?? null;
}
