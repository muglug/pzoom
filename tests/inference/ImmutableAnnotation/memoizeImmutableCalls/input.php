<?php
function takesString(string $s) : void {}

/**
 * @psalm-immutable
 */
class DTO {
    /** @var string|null */
    private $error;

    public function __construct(?string $error) {
        $this->error = $error;
    }

    public function getError(): ?string {
        return $this->error;
    }
}

$dto = new DTO("BOOM!");

if ($dto->getError() !== null) {
    takesString($dto->getError());
}
