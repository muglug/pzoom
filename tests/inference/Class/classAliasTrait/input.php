<?php
trait FeatureV1 {}
class_alias(FeatureV1::class, Feature::class);
class App { use Feature; }
