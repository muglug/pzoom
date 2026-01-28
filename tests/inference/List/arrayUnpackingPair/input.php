<?php

class PseudoMethodStorage {
    /** @return array */
    public function getParams(): array {
        return [];
    }
}

class ClassLikeStorage {
    public function getName(): string {
        return "";
    }
}

$found_method_and_class_storage = [new PseudoMethodStorage(), new ClassLikeStorage()];
[$pseudo_method_storage, $defining_class_storage] = $found_method_and_class_storage;

$pseudo_method_storage->getParams();
$defining_class_storage->getName();
