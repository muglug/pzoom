<?php
class A{
    /**
     * @template T
     * @param array<T> $objects
     * @return array<T>
     */
    private function uniquateObjects(array $objects) : array
    {
        $uniqueObjects = [];
        foreach ($objects as $object) {
            if (in_array($object, $uniqueObjects, true)) {
                continue;
            }
            $uniqueObjects[] = $object;
        }

        return $uniqueObjects;
    }
}
