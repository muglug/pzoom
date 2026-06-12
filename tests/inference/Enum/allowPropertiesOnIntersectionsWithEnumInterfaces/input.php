<?php
interface I {}

interface UE extends UnitEnum {}
interface BE extends BackedEnum {}

function f(I $i): void {
    if ($i instanceof BackedEnum) {
        echo $i->name;
    }
    if ($i instanceof UnitEnum) {
        echo $i->name;
    }
    if ($i instanceof UE) {
        echo $i->name;
    }
    if ($i instanceof BE) {
        echo $i->name;
    }
}
