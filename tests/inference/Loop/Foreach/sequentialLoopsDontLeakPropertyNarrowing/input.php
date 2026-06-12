<?php
class Loc3 {}
class CS3 { public ?Loc3 $location = null; }
function emit3(Loc3 $_l): void {}
/** @param list<string> $ifaces @param list<string> $parents */
function check3(array $ifaces, array $parents, CS3 $const_storage): void {
    $interface_const_storage = null;
    $parent_classlike_storage = null;
    foreach ($ifaces as $interface) {
        $parent_const = rand(0,1) ? new CS3 : null;
        if ($parent_const !== null) {
            if ($const_storage->location
                && $const_storage !== $parent_const
                && rand(0,1)
            ) {
                emit3($const_storage->location);
            }
            if ($interface_const_storage !== null && $const_storage->location !== null) {
                assert($parent_classlike_storage !== null);
                emit3($const_storage->location);
                echo $interface;
            }
            $interface_const_storage = $parent_const;
            $parent_classlike_storage = $interface;
        }
    }
    foreach ($parents as $parent_class) {
        $parent_const = rand(0,1) ? new CS3 : null;
        if ($parent_const !== null) {
            if ($const_storage->location !== null && $interface_const_storage !== null) {
                assert($parent_classlike_storage !== null);
                emit3($const_storage->location);
                echo $parent_class;
            }
            $parent_classlike_storage = $parent_class;
            break;
        }
    }
}
