<?php
final class O {}
final class C {
    private bool $a = false;
    public array $_types = [];

    private static function mirror(array $a) : array {
        return $a;
    }

    /**
     * @param class-string<O>|null $type
     */
    public function addType(?string $type, array $ids = array()): void
    {
        if ($this->a) {
            $ids = self::mirror($ids);
        }
        $this->_types[$type ?: ""] = new ArrayObject($ids);
        return;
    }
}

(new C)->addType(null);
