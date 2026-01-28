<?php
/**
 * @property array<string, string> $errors
 *
 * @psalm-seal-properties
 */
final class OrganizationObject {

    public function __get(string $key)
    {
        return [];
    }

    /**
     * @param mixed $a
     */
    public function __set(string $key, $a): void
    {
    }

    public function updateErrors(): void {
        /** @var array<string, string> */
        $errors = [];
        $this->errors = $errors;
    }
    /** @return array<string, string> */
    public function updateStatus(): array {
        $_ = $this->errors;
        $this->updateErrors();
        $errors = $this->errors;
        return $errors;
    }
}
