pub fn load_init_user() {
    mene_syscall::spawn_app("/boot/init");
}
