<?php
final class Settings {
    public bool $from_docblock = false;
    public ?string $initialized_class = null;
    protected int $hidden = 0;
}

/** @return array{from_docblock: bool, initialized_class: null|string} */
function snapshot(Settings $s): array {
    return get_object_vars($s);
}
