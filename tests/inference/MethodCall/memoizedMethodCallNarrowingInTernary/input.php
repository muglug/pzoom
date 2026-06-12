<?php
class Ex2 {
    /** @psalm-pure */
    public function getPrevious(): ?Ex2 { return null; }
    public function getCode(): int { return 0; }
}

function codeOf(Ex2 $exception): ?int {
    return $exception->getPrevious()
        ? $exception->getPrevious()->getCode()
        : null;
}
