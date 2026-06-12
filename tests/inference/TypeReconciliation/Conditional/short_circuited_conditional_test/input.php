<?php
/** @var ?stdClass $existing */
$existing = null;

/** @var bool $foo */
$foo = true;

if ($foo) {
} elseif ($existing === null) {
    throw new \RuntimeException();
}
