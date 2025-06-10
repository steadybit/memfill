pub fn linux_main(){
    adjust_oom_score();

	let mem_info: Box<dyn MemInfoProvider> = if opts.ignore_cgroup {
		Box::new(SystemMemInfo {})
	} else {
		Box::new(CgroupMemInfo {})
	};

}


fn adjust_oom_score() -> () {
	let is_privileged = Uid::current().is_root() || Uid::effective().is_root();
	match fs::write("/proc/self/oom_score_adj", if is_privileged { "-1000" } else { "0" }) {
		Ok(_) => {}
		Err(e) => {
			eprintln!("Failed to adjust OOM score: {}", e);
		}
	}
}