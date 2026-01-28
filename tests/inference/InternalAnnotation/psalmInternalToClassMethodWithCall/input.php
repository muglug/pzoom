<?php
                    namespace A\B {
                        class Foo {
                            /**
                             * @psalm-internal A\B\Foo
                             */
                            public static function barBar(): void {
                            }
                        }
                    }

                    namespace A\C {
                        class Bat {
                            public function batBat(): void {
                                \A\B\Foo::barBar();
                            }
                        }
                    }
