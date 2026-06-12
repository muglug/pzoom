<?php
final class Cfg { /** @var int<1, max>|null */ public ?int $threads = null; }
function f(Cfg $config, int $n): void {
    $config->threads = max(1, $n);
}
