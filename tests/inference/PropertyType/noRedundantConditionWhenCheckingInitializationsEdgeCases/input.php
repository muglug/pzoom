<?php
final class Clazz {
    /**
     * @var bool
     */
    public $x;

    /**
     * @var int
     */
    public $y = 0;

    public function func1 (): bool {
        if ($this->y !== 0) {
            return true;
        }
        return false;
    }

    public function func2 (): int {
        if ($this->y !== 0) {
            return $this->y;
        }
        return 2;
    }

    public function __construct () {
        $this->x = false;
        if ($this->func1()) {
            $this->y = $this->func2();
        }
        $this->func2();
    }
}
