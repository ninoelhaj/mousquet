// L'icône de l'exe (le tigre) : assets\mousquet.ico, intégrée par le compilateur de ressources de Windows.
fn main() {
    println!("cargo:rerun-if-changed=mousquet.rc");
    println!("cargo:rerun-if-changed=assets/mousquet.ico");
    embed_resource::compile("mousquet.rc", embed_resource::NONE).manifest_optional().unwrap();
}
