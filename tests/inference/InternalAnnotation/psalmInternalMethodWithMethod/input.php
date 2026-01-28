<?php
                    namespace X {
                        class Foo {
                            /**
                             * @psalm-internal Y\Bat::batBat
                             */
                            public static function barBar(): void {
                            }
                        }
                    }

                    namespace Y {
                        class Bat {
                            public function batBat() : void {
                                \X\Foo::barBar();
                            }
                        }
                    }
