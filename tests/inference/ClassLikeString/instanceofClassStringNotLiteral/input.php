<?php
    final class Z {
    /**
     * @psalm-var class-string<stdClass> $class
     */
    private string $class = stdClass::class;

    public function go(object $object): ?stdClass {
        $a = $this->class;
        if ($object instanceof $a) {
            return $object;
        }
        return null;
    }
}
