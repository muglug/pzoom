<?php
$list = [3, 2, 5, 9];
usort($list, fn(int $a, string $b): int => (int) ($a > $b));
