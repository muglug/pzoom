<?php
class C {
    /**
     * @param mixed $p
     * @psalm-assert-if-true int $p
     */
    public function isInt($p): bool {
        return is_int($p);
    }
    /**
     * @param mixed $p
     */
    public function doWork($p): void {
        if ($this->isInt($p)) {
            strlen($p);
        }
    }
}
