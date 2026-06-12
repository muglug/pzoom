<?php
class Sig30 { public function equals(Sig30 $o): bool { return $this === $o; } }
class Prop30 { public ?Sig30 $signature_type = null; public ?string $location = null; }

function f(Prop30 $property_storage, Prop30 $guide_property_storage): void {
    if ((($property_storage->signature_type && !$guide_property_storage->signature_type)
            || (!$property_storage->signature_type && $guide_property_storage->signature_type)
            || ($property_storage->signature_type
                && !$property_storage->signature_type->equals(
                    $guide_property_storage->signature_type,
                )))
        && $property_storage->location
    ) {
        echo $property_storage->location;
    }
}
