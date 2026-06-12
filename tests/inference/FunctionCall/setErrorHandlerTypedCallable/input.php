<?php
class ConfigTest2 {
    /** @var callable(int, string, string=, int=, array<array-key, mixed>=):bool|null */
    private $original_error_handler = null;

    public function setUp(): void {
        $this->original_error_handler = set_error_handler(null);
    }

    public function check(): bool {
        return $this->original_error_handler !== null;
    }
}
