<?php
/** @psalm-suppress InvalidScope */
if (!isset($this->value)) {
    $this->value = ["x", "y"];
    echo count($this->value) - 2;
}