<?php
/** @psalm-suppress UnimplementedInterfaceMethod, MissingTemplateParam */
final class IterableResult implements \Iterator {
    public function current() {
        return null;
    }

    public function key() {
        return 5;
    }
}

$items = new IterableResult();

foreach ($items as $_item) {}
