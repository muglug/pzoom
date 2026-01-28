<?php
/**
 * @template T as object
 */
class Mapper {
    /**
     * @param T $e
     * @return T
     */
    public function foo($e) {
        return $e;
    }

    /**
     * @param T $e
     * @return T
     */
    public function passthru($e) {
        return $this->foo($e);
    }
}