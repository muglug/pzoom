<?php
class Dep { public string $name = ''; /** @var array<string, bool> */ public array $dependent_classlikes = []; }
function getDep(string $_name): Dep { return new Dep(); }
function walk(Dep $dependency, string $key): void {
    $dependencies = [strtolower($dependency->name) => true];
    do {
        $current_dependency_name = key(array_splice($dependencies, 0, 1));
        $current_dependency = getDep($current_dependency_name);
        $dependencies += $current_dependency->dependent_classlikes;
    } while (!isset($dependencies[$key]) && $dependencies !== []);
}
