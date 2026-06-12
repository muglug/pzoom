<?php

class ClassLikeStorage {
    /** @var array<string, array<string, string>> */
    public array $pseudo_property_set_types = [];
}

class Codebase {
    /** @var array<string, ClassLikeStorage> */
    public array $storages = [];

    public function getStorage(string $name): ?ClassLikeStorage
    {
        return $this->storages[$name] ?? null;
    }

    public function check(string $fq_class_name, string $property_name): ?string
    {
        $class_storage = $this->getStorage($fq_class_name);

        if (isset($class_storage->pseudo_property_set_types['$' . $property_name])) {
            return $class_storage->pseudo_property_set_types['$' . $property_name]['x'] ?? null;
        }

        return null;
    }
}
